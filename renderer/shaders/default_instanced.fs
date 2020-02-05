#version 140

precision mediump float;

uniform vec4 lightColor;
uniform vec4 lightDirection;

/* material */
const vec3 ambient = vec3(0.2, 0.2, 0.2);
const vec3 emissive = vec3(0.1, 0.1, 0.1);
const vec3 specular = vec3(1.0, 1.0, 1.0);
const float shininess = 75.0;
const vec3 ambientLightColor = vec3(0.3, 0.3, 0.3);
const float specularStrength = 1.0;

const bool isBfcCertified = ##IS_BFC_CERTIFIED##;

varying vec3 vViewPosition;
varying vec3 vNormal;
varying vec4 vColor;
varying mat4 vInvertedView;

void main() {
    if (!isBfcCertified) {
        gl_FragColor = vColor;
        return;
    }

    vec3 diffuse = vColor.xyz;

    gl_FragColor = vec4( 1.0, 1.0, 1.0, vColor.w );

    vec3 viewPosition = normalize( vViewPosition );

    vec3 dirDiffuse  = vec3( 0.0 );
    vec3 dirSpecular = vec3( 0.0 );
    vec4 lDirection = vInvertedView * vec4( lightDirection.xyz, 0.0 );
    vec3 dirVector = normalize( lDirection.xyz );
    float dotProduct = dot( vNormal, dirVector );
    float dirDiffuseWeight = max( dotProduct, 0.0 );

    dirDiffuse  += diffuse * lightColor.xyz * dirDiffuseWeight;
    vec3 dirHalfVector = normalize( dirVector + viewPosition );
    float dirDotNormalHalf = max( dot( vNormal, dirHalfVector ), 0.0 );
    float dirSpecularWeight = specularStrength * max( pow( dirDotNormalHalf, shininess ), 0.0 );

    dirSpecular += specular * lightColor.xyz * dirSpecularWeight * dirDiffuseWeight;

    gl_FragColor.xyz = gl_FragColor.xyz * ( emissive + dirDiffuse + ambientLightColor * ambient ) + dirSpecular;
    gl_FragColor.w = gl_FragColor.w + (dirSpecular.x + dirSpecular.y + dirSpecular.z) / 3.0;
}
